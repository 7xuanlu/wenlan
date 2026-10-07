// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowLeft, ArrowRight } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  listMemoriesRich,
  listSpaces,
  openSearchResult as openSearchResultTarget,
  searchEntities,
  searchPages,
  takeRemotePairingLink,
  type Page,
  type SearchResult,
  type Space,
} from "../../lib/tauri";
import { clearPendingPairingCode, setPendingPairingCode, usePendingPairingCode } from "../../lib/pairingLink";
import { MAIN_HEADER_HEIGHT, topBarLeftInset } from "../../lib/windowChrome";
import ActivityFeed from "./ActivityFeed";
import ActivityStatus from "./activity/ActivityStatus";
import { useSearch } from "../../hooks/useSearch";
import EntityDetail from "./EntityDetail";
import MemoryStream from "./MemoryStream";
import type { SortMode } from "./MemoryStream";
import { FirstUseGuide } from "../onboarding/FirstUseGuide";
import AtlasView from "./AtlasView";
import MemorySearchResult from "./MemorySearchResult";
import MemoryDetail from "./MemoryDetail";
import PageDetail from "./PageDetail";
import DistillReviewPanel from "./DistillReviewPanel";
import SettingsPage from "./SettingsPage";
import { ImportView } from "./ImportView";
import { SetupWizard } from "../SetupWizard";
import Sidebar, { SidebarToggleButton } from "./Sidebar";
import SettingsSidebar from "./settings/SettingsSidebar";
import SpaceDetail from "./SpaceDetail";
import { SpacesOverview } from "./spaces";
import { PagesOverview } from "./pages/PagesOverview";
import { EntitiesView } from "./entities/EntitiesView";
import {
  PageDraftEditor,
  type PageDraftEditorHandle,
} from "./pages/PageDraftEditor";
import SourcesView from "./SourcesView";
import type { SourceLibraryState } from "./sources/SourceLibrary";
import { RecapsList } from "./RecapsList";
import AboutWenlanDialog from "./AboutWenlanDialog";
import { readPreference, writePreference } from "../../lib/preferenceStorage";
import { searchResultTarget } from "../../lib/searchResultTarget";
import { recordRecentPageVisit } from "../../lib/recentPages";
import { deleteRecentSpace, recordRecentSpaceVisit, renameRecentSpace } from "../../lib/recentSpaces";
import { createSpaceDetailCopy, createSpacesOverviewLabels } from "./navigation/copy";
import { activeNavigationForView, type View } from "./navigation/viewState";
import { ReviewEnvironmentBadge } from "./navigation/ReviewEnvironmentBadge";
import QuickCaptureScrim from "./QuickCaptureScrim";
import { useResponsiveSidebar } from "./navigation/useResponsiveSidebar";
import { useLaunchPinFill } from "../../lib/launchPinFill";
import type { MarkdownEditorSelection } from "./editor/MarkdownEditor";
import { useViewScroll } from "./navigation/useViewScroll";
import { WorkspaceBackButton, WorkspaceNavigationProvider } from "./navigation/WorkspaceNavigation";
import "./navigation/navigation-shell.css";
import { inventoryFolderPath } from "./pages/pageInventory";

interface MainProps {
  initialView?: View;
  initialMemoryId?: string | null;
  initialPageId?: string | null;
  onBackFromDetail?: () => void;
  onRegisterQuitGuard?: (guard: (() => Promise<boolean>) | null) => void;
}
const SIDEBAR_KEY = "wenlan-sidebar-collapsed";
const LEGACY_SIDEBAR_KEY = "origin-sidebar-collapsed";

function scrollDestinationKey(view: View): string {
  switch (view.kind) {
    case "entity":
      return `entity:${view.entityId}`;
    case "memory":
      return `memory:${view.sourceId}`;
    case "page":
      return `page:${view.pageId}`;
    case "page-draft":
      return `page-draft:${view.draftId ?? "new"}:${view.space ?? "none"}`;
    case "settings":
      return `settings:${view.section ?? "general"}`;
    case "space":
      return `space:${view.spaceId ?? view.spaceName}`;
    default:
      return view.kind;
  }
}

// Compare destinations rather than object identity. A fresh unsaved draft is
// always a new intent; persisted drafts retain their session through history.
function sameDestination(current: View, next: View): boolean {
  if ((current.kind === "home" || current.kind === "pages")
    && (next.kind === "home" || next.kind === "pages")) {
    return (current.kind === "pages" ? current.inventoryScope ?? "all" : "all")
      === (next.kind === "pages" ? next.inventoryScope ?? "all" : "all");
  }
  if (current.kind !== next.kind) return false;
  switch (current.kind) {
    case "page": return next.kind === "page" && current.pageId === next.pageId
      && (current.mode ?? "edit") === (next.mode ?? "edit");
    case "memory": return next.kind === "memory" && current.sourceId === next.sourceId;
    case "entity": return next.kind === "entity" && current.entityId === next.entityId;
    case "settings": return next.kind === "settings"
      && (current.section ?? "general") === (next.section ?? "general");
    case "space": return next.kind === "space" && (current.spaceId && next.spaceId
      ? current.spaceId === next.spaceId : current.spaceName === next.spaceName);
    case "spaces": return next.kind === "spaces" && !!current.create === !!next.create;
    case "import": return next.kind === "import" && !!current.fromFirstUse === !!next.fromFirstUse;
    case "first-use": return next.kind === "first-use"
      && !!current.showKnowledge === !!next.showKnowledge && current.batchId === next.batchId;
    case "page-draft": return next.kind === "page-draft" && current.space === next.space
      && (next.sessionKey != null ? current.sessionKey === next.sessionKey
        : !!next.draftId && current.draftId === next.draftId);
    default: return true;
  }
}

export default function Main({
  initialView,
  initialMemoryId,
  initialPageId,
  onBackFromDetail,
  onRegisterQuitGuard,
}: MainProps) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  // Give the unpinned routing jobs a source once per launch. Main is the first
  // screen a set-up user reaches, and the only fills before this one lived in
  // the webview, so a quit during the model download left both jobs blank
  // forever. See src/lib/launchPinFill.ts.
  useLaunchPinFill();
  const mainContentRef = useRef<HTMLElement>(null);
  const pageDraftEditorRef = useRef<PageDraftEditorHandle>(null);
  const nextDraftSessionRef = useRef(0);
  const pendingDraftNavigationRef = useRef<{
    readonly action: (sourceView: View) => void;
    readonly token: symbol;
  } | null>(null);
  const draftNavigationFlushRef = useRef<Promise<boolean> | null>(null);
  const pendingDraftSearchCancelRef = useRef<(() => void) | null>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const sidebarToggleRef = useRef<HTMLButtonElement>(null);
  const initialPageRequestRef = useRef<{
    observed: string | null;
    pending: string | null;
  }>({ observed: initialPageId ?? null, pending: null });
  const initialMemoryRequestRef = useRef<{
    observed: string | null;
    active: string | null;
    pending: { memoryId: string | null } | null;
  }>({
    observed: initialMemoryId ?? null,
    active: initialMemoryId ?? null,
    pending: null,
  });
  const pageFlushRef = useRef<(() => Promise<boolean>) | null>(null);
  const pageNavigationTokenRef = useRef<symbol | null>(null);
  const registerPageFlush = useCallback((flush: (() => Promise<boolean>) | null) => {
    pageFlushRef.current = flush;
    // A remembered View object may be reused by Back/Notes. Readiness belongs
    // to the mounted editor, so a departed editor cannot unlock restoration.
    if (!flush) setReadyEditorView(null);
  }, []);
  const pageSavePendingRef = useRef(false);
  const pageEditDirtyRef = useRef(false);
  const [view, setView] = useState<View>(
    initialMemoryId ? { kind: "memory", sourceId: initialMemoryId }
    : initialPageId ? { kind: "page", pageId: initialPageId }
    : initialView ?? { kind: "pages" },
  );
  const viewRef = useRef(view);
  viewRef.current = view;
  const [viewHistory, setViewHistory] = useState<View[]>([]);
  const viewHistoryRef = useRef<View[]>([]);
  const [viewForward, setViewForward] = useState<View[]>([]);
  const viewForwardRef = useRef<View[]>([]);
  const updateHistory = (past: View[], forward: View[]) => {
    viewHistoryRef.current = past;
    viewForwardRef.current = forward;
    setViewHistory(past);
    setViewForward(forward);
  };
  const dismissSearch = () => {
    pendingDraftSearchCancelRef.current?.();
    pendingDraftSearchCancelRef.current = null;
    setPendingDraftSearchQuery(null);
    if (query) setQuery("");
    setMobileSearchOpen(false);
  };
  const showView = (next: View, dismissSearchOverlay = true) => {
    if (dismissSearchOverlay) dismissSearch();
    viewRef.current = next;
    setView(next);
    if (next.kind === "pages" || next.kind === "home") setActiveTab("pages");
    else if (next.kind === "activity") setActiveTab("activity");
  };
  // Identity promotion and completed workflows replace the current entry.
  // Their abandoned forward branch must not restore a superseded editor.
  const replaceView = (next: View) => {
    updateHistory(viewHistoryRef.current, []);
    showView(next);
  };
  const lastNoteViewRef = useRef<View | null>(null);
  if (view.kind === "page" || view.kind === "page-draft") lastNoteViewRef.current = view;
  const pageSelectionsRef = useRef(new Map<string, MarkdownEditorSelection>());
  const [readyEditorView, setReadyEditorView] = useState<View | null>(null);
  const [sourceLibraryState, setSourceLibraryState] = useState<SourceLibraryState>({ search: "", filter: "all" });
  const contextSpace = viewHistory.slice().reverse().find(
    (item): item is Extract<View, { kind: "space" }> => item.kind === "space",
  );
  const [activeTab, setActiveTab] = useState<"pages" | "activity">(view.kind === "activity" ? "activity" : "pages");
  // The Activity button's summary. Owned here, beside the toolbar that renders
  // it, so the toggle keeps a stable identity for the outside-click listener.
  const [activityOpen, setActivityOpen] = useState(false);
  const toggleActivity = useCallback(() => setActivityOpen((open) => !open), []);
  const [aboutOpen, setAboutOpen] = useState(false);
  const memoryNavigationGuardRef = useRef<(() => boolean) | null>(null);
  const registerMemoryNavigationGuard = useCallback((guard: (() => boolean) | null) => {
    memoryNavigationGuardRef.current = guard;
  }, []);
  const [pageSavePending, setPageSavePending] = useState(false);
  const [pageEditDirty, setPageEditDirty] = useState(false);
  const [recentPagesRevision, setRecentPagesRevision] = useState(0);
  const [recentSpacesRevision, setRecentSpacesRevision] = useState(0);
  const [mobileSearchOpen, setMobileSearchOpen] = useState(false);
  const [pendingDraftSearchQuery, setPendingDraftSearchQuery] = useState<string | null>(null);
  const viewScrollDestination = scrollDestinationKey(view);
  pageSavePendingRef.current = pageSavePending;
  pageEditDirtyRef.current = pageEditDirty;

  const canLeaveCurrentPage = () => {
    if (viewRef.current.kind === "memory" && memoryNavigationGuardRef.current && !memoryNavigationGuardRef.current()) return false;
    if (pageSavePending) return false;
    if (
      view.kind === "page"
      && pageEditDirty
      && !confirm(t("pageDetail.editor.discardConfirm"))
    ) {
      return false;
    }
    if (pageEditDirty) setPageEditDirty(false);
    return true;
  };

  const prepareForQuit = useCallback(async (): Promise<boolean> => {
    if (viewRef.current.kind === "memory" && memoryNavigationGuardRef.current) return memoryNavigationGuardRef.current();
    if (viewRef.current.kind === "page" && pageFlushRef.current) {
      try { return await pageFlushRef.current(); } catch { return false; }
    }
    if (pageSavePendingRef.current || pageEditDirtyRef.current) return false;
    const editor = pageDraftEditorRef.current;
    if (viewRef.current.kind !== "page-draft" || !editor) return true;
    return editor.flush();
  }, []);

  useEffect(() => {
    onRegisterQuitGuard?.(prepareForQuit);
    return () => onRegisterQuitGuard?.(null);
  }, [onRegisterQuitGuard, prepareForQuit]);

  useViewScroll(mainContentRef, viewScrollDestination,
    view.kind !== "page" || view.mode === "read" || readyEditorView === view);

  const afterPageDraftFlush = (action: (sourceView: View) => void): (() => void) => {
    const editor = pageDraftEditorRef.current;
    if (view.kind !== "page-draft" || !editor) {
      action(view);
      return () => {};
    }

    const token = Symbol("draft-navigation");
    pendingDraftNavigationRef.current = { action, token };
    const cancel = () => {
      if (pendingDraftNavigationRef.current?.token === token) {
        pendingDraftNavigationRef.current = null;
      }
    };
    if (draftNavigationFlushRef.current) return cancel;

    const flush = editor.flush();
    draftNavigationFlushRef.current = flush;
    void flush.then(
      (saved) => {
        const pending = pendingDraftNavigationRef.current;
        pendingDraftNavigationRef.current = null;
        if (saved && pending) {
          const identity = editor.getIdentity();
          const sourceView: View = {
            ...view,
            draftId: identity.draftId ?? undefined,
          };
          // The intermediate setView may be batched away by navigation.
          // Remember the saved identity before leaving so Notes reopens it.
          lastNoteViewRef.current = sourceView;
          if (sourceView !== view) showView(sourceView, false);
          pending.action(sourceView);
        }
      },
      () => {
        pendingDraftNavigationRef.current = null;
      },
    ).finally(() => {
      if (draftNavigationFlushRef.current === flush) {
        draftNavigationFlushRef.current = null;
      }
    });
    return cancel;
  };

  const afterNavigationGuards = (
    action: (sourceView: View) => void,
    onRefused?: () => void,
  ): (() => void) => {
    const flush = view.kind === "page" ? pageFlushRef.current : null;
    if (flush) {
      const sourceView = view;
      const token = Symbol("page-navigation");
      pageNavigationTokenRef.current = token;
      void flush().then((saved) => {
        if (pageNavigationTokenRef.current !== token || viewRef.current !== sourceView) return;
        pageNavigationTokenRef.current = null;
        if (saved) action(sourceView);
        else onRefused?.();
      }, () => {
        if (pageNavigationTokenRef.current !== token) return;
        pageNavigationTokenRef.current = null;
        onRefused?.();
      });
      return () => {
        if (pageNavigationTokenRef.current === token) pageNavigationTokenRef.current = null;
      };
    }
    if (!canLeaveCurrentPage()) {
      onRefused?.();
      return () => {};
    }
    return afterPageDraftFlush(action);
  };

  useEffect(() => {
    const request = initialPageRequestRef.current;
    const next = initialPageId ?? null;
    if (request.observed !== next) {
      request.observed = next;
      request.pending = next;
    }
    const target = request.pending;
    if (!target || pageSavePending) return;
    request.pending = null;
    if (
      (view.kind === "page" && view.pageId === target)
    ) {
      return;
    }
    const cancel = afterNavigationGuards(() => replaceView({ kind: "page", pageId: target }));
    // An autosave changes pending state while this navigation waits. Do not
    // cancel the requested destination merely because that state changed.
    return pageFlushRef.current ? undefined : cancel;
  }, [initialPageId, pageSavePending]);

  useEffect(() => {
    const request = initialMemoryRequestRef.current;
    const next = initialMemoryId ?? null;
    if (request.observed !== next) {
      request.observed = next;
      request.pending = next || request.active ? { memoryId: next } : null;
    }
    const target = request.pending;
    if (!target || pageSavePending) return;
    request.pending = null;
    if (target.memoryId) {
      if (
        view.kind === "memory"
        && view.sourceId === target.memoryId
      ) {
        request.active = target.memoryId;
        return;
      }
    } else if (view.kind === activeTab) {
      request.active = null;
      return;
    }
    const cancel = afterNavigationGuards(() => {
      request.active = target.memoryId;
      updateHistory([], []);
      showView(
        target.memoryId
          ? { kind: "memory", sourceId: target.memoryId }
          : { kind: activeTab },
      );
    });
    return pageFlushRef.current ? undefined : cancel;
  }, [initialMemoryId, activeTab, pageSavePending]);

  // New destinations branch from the current entry only after saving succeeds.
  const navigateTo = (next: View, onRefused?: () => void) => {
    if (sameDestination(viewRef.current, next)) {
      if (query || pendingDraftSearchQuery || mobileSearchOpen) {
        afterNavigationGuards(dismissSearch, onRefused);
      }
      return;
    }
    afterNavigationGuards((sourceView) => {
      updateHistory([...viewHistoryRef.current, sourceView], []);
      // Allocate only after the current editor's save guard succeeds.
      showView(next.kind === "page-draft" && next.sessionKey == null
        ? { ...next, sessionKey: ++nextDraftSessionRef.current }
        : next);
    }, onRefused);
  };

  const navigateHome = () => navigateTo({ kind: "pages" });
  const navigateSpaces = (create: boolean) => navigateTo(create
    ? { kind: "spaces", create: true } : { kind: "spaces" });
  const navigatePages = () => navigateTo({ kind: "pages" });
  const navigateNotes = () => {
    if (activeNavigationForView(view) !== "pages" && lastNoteViewRef.current) {
      navigateTo(lastNoteViewRef.current);
    } else {
      navigatePages();
    }
  };

  const externalDetailExit = view.kind === "memory" && !!initialMemoryId
    && !!onBackFromDetail && viewHistory.length === 0;
  const fallbackView: View = view.kind === "space" ? { kind: "spaces" } : { kind: activeTab };
  const canNavigateBack = viewHistory.length > 0 || externalDetailExit || !sameDestination(view, fallbackView);
  const applyBackNavigation = (sourceView: View) => {
    const past = viewHistoryRef.current;
    if (past.length === 0) {
      if (sourceView.kind === "memory" && initialMemoryId && onBackFromDetail) {
        onBackFromDetail();
        return;
      }
      const fallback: View = sourceView.kind === "space" ? { kind: "spaces" } : { kind: activeTab };
      if (sameDestination(sourceView, fallback)) return;
      updateHistory([], [...viewForwardRef.current, sourceView]);
      showView(fallback);
      return;
    }
    updateHistory(past.slice(0, -1), [...viewForwardRef.current, sourceView]);
    showView(past[past.length - 1]);
  };
  const navigateBack = () => { afterNavigationGuards(applyBackNavigation); };
  const navigateForward = () => {
    if (viewForwardRef.current.length === 0) return;
    afterNavigationGuards((sourceView) => {
      const forward = viewForwardRef.current;
      if (!forward.length) return;
      updateHistory([...viewHistoryRef.current, sourceView], forward.slice(0, -1));
      showView(forward[forward.length - 1]);
    });
  };

  // Legacy standalone PageDetail confirms before calling this callback.
  const navigateBackFromPageDetail = () => {
    if (pageFlushRef.current) {
      navigateBack();
      return;
    }
    if (pageSavePending) return;
    setPageEditDirty(false);
    applyBackNavigation(viewRef.current);
  };
  const [sortMode, setSortMode] = useState<SortMode>("recent");
  const [stabilityFilter, setStabilityFilter] = useState<string | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => {
    return readPreference(SIDEBAR_KEY, LEGACY_SIDEBAR_KEY) === "true";
  });
  const { query, setQuery, debouncedQuery, results } = useSearch();
  const handleSearchQueryChange = (nextQuery: string) => {
    const needsFlush = view.kind === "page-draft" || (view.kind === "page" && !!pageFlushRef.current);
    if (!query && nextQuery && !needsFlush && !canLeaveCurrentPage()) return;
    if (!query && nextQuery && needsFlush) {
      pendingDraftSearchCancelRef.current?.();
      setPendingDraftSearchQuery(nextQuery);
      pendingDraftSearchCancelRef.current = afterNavigationGuards(() => {
        pendingDraftSearchCancelRef.current = null;
        setQuery(nextQuery);
        setPendingDraftSearchQuery(null);
      }, () => {
        pendingDraftSearchCancelRef.current = null;
        setPendingDraftSearchQuery(null);
      });
      return;
    }
    pendingDraftSearchCancelRef.current?.();
    pendingDraftSearchCancelRef.current = null;
    setPendingDraftSearchQuery(null);
    setQuery(nextQuery);
  };
  const displayedSearchQuery = pendingDraftSearchQuery ?? query;
  const memoryResults = results.filter((result) => searchResultTarget(result).kind === "copy");
  const sourceResults = results.filter((result) => searchResultTarget(result).kind === "file");

  useEffect(() => {
    if (mobileSearchOpen && !pageSavePending) searchInputRef.current?.focus();
  }, [mobileSearchOpen, pageSavePending]);

  useEffect(() => {
    if (view.kind !== "page-draft" && view.kind !== "page") setPendingDraftSearchQuery(null);
  }, [view.kind]);

  const { data: entityResults = [] } = useQuery({
    queryKey: ["searchEntities", debouncedQuery],
    queryFn: () => searchEntities(debouncedQuery, 5),
    enabled: debouncedQuery.length > 0,
  });

  const { data: conceptResults = [] } = useQuery({
    queryKey: ["searchPages", debouncedQuery],
    queryFn: () => searchPages(debouncedQuery, 5),
    enabled: debouncedQuery.length > 0,
  });

  const toggleSidebar = () => {
    setSidebarCollapsed((v) => {
      const next = !v;
      writePreference(SIDEBAR_KEY, String(next));
      return next;
    });
  };
  const activeNavigation = activeNavigationForView(view);
  const browseContext = activeNavigation !== null && activeNavigation !== "pages" && activeNavigation !== "home" && activeNavigation !== "memories";
  const [browseSidebarOpen, setBrowseSidebarOpen] = useState<Record<string, boolean>>({});
  const contextSidebarCollapsed = browseContext ? !browseSidebarOpen[activeNavigation] : sidebarCollapsed;
  const toggleContextSidebar = () => {
    if (browseContext) setBrowseSidebarOpen((current) => ({ ...current, [activeNavigation]: !current[activeNavigation] }));
    else toggleSidebar();
  };
  const responsiveSidebar = useResponsiveSidebar(contextSidebarCollapsed, toggleContextSidebar, sidebarToggleRef);
  const standardSidebarMounted = view.kind !== "settings" && view.kind !== "connect-agent";
  const spacesOverviewLabels = createSpacesOverviewLabels(t);
  const spaceDetailCopy = createSpaceDetailCopy(t);
  const { data: spaces } = useQuery({ queryKey: ["spaces"], queryFn: listSpaces });

  const refreshRecentSpaces = useCallback(() => {
    setRecentSpacesRevision((revision) => revision + 1);
  }, []);
  const handlePageLoaded = useCallback((page: Pick<Page, "id" | "status" | "title">) => {
    recordRecentPageVisit(page);
    setRecentPagesRevision((revision) => revision + 1);
  }, []);
  const handleSpaceLoaded = useCallback((space: Space) => {
    const runtime = spaces === undefined
      ? undefined
      : { spaces: [space, ...spaces.filter((current) => current.id !== space.id)] };
    recordRecentSpaceVisit(space, runtime);
    setView((current) => current.kind === "space" && current.spaceName === space.name
      ? { ...current, spaceId: space.id }
      : current);
    refreshRecentSpaces();
  }, [refreshRecentSpaces, spaces]);
  const handleSpaceRenamed = useCallback((space: Pick<Space, "id" | "name">) => {
    const runtime = spaces === undefined
      ? undefined
      : {
          spaces: spaces.map((current) => current.id === space.id
            ? { ...current, name: space.name }
            : current),
        };
    renameRecentSpace(space, runtime);
    setView((current) => current.kind === "space" && current.spaceId === space.id
      ? { ...current, spaceName: space.name }
      : current);
    refreshRecentSpaces();
  }, [refreshRecentSpaces, spaces]);
  const handleSpaceDeleted = useCallback((spaceId: string) => {
    const runtime = spaces === undefined
      ? undefined
      : { spaces: spaces.filter(({ id }) => id !== spaceId) };
    deleteRecentSpace(spaceId, runtime);
    refreshRecentSpaces();
  }, [refreshRecentSpaces, spaces]);

  const { data: memories = [] } = useQuery({
    queryKey: ["memories"],
    queryFn: () => listMemoriesRich(undefined, undefined, undefined, 200),
    refetchInterval: view.kind === "stream" ? 5000 : false,
  });

  // Listen for capture events
  useEffect(() => {
    const unlisten = listen<{ source: string }>("capture-event", () => {
      queryClient.invalidateQueries({ queryKey: ["memories"] });
      queryClient.invalidateQueries({ queryKey: ["memoryStats"] });
      queryClient.invalidateQueries({ queryKey: ["recentChanges"] });
      queryClient.invalidateQueries({ queryKey: ["recentRetrievals"] });
      queryClient.invalidateQueries({ queryKey: ["spaces"] });
    });
    return () => { unlisten.then((f) => f()); };
  }, [queryClient]);

  // "Open in Wenlan" on the relay pairing page sends a wenlan://pair link. The
  // native side holds the code until asked, so a link that launched the app is
  // not lost before this listener exists.
  useEffect(() => {
    const pull = () => {
      takeRemotePairingLink()
        .then((code) => { if (code) setPendingPairingCode(code); })
        .catch(() => {});
    };
    pull();
    const unlisten = listen("remote-pairing-link", pull);
    return () => { unlisten.then((f) => f()); };
  }, []);

  // The Connections panel takes the code; it stays pending until the panel is
  // shown. Keeping unsaved page edits drops the link rather than parking it.
  const pendingPairingCode = usePendingPairingCode();
  useEffect(() => {
    if (!pendingPairingCode || pageSavePending) return;
    if (view.kind === "settings" && view.section === "agents") return;
    navigateTo({ kind: "settings", section: "agents" }, clearPendingPairingCode);
  }, [pendingPairingCode, pageSavePending]);

  // Cmd+K global shortcut (fired from App.tsx) — focus the header search input.
  useEffect(() => {
    const unlisten = listen("focus-search", () => {
      if (pageSavePending) return;
      setMobileSearchOpen(true);
      searchInputRef.current?.focus();
      searchInputRef.current?.select();
    });
    return () => { unlisten.then((f) => f()); };
  }, [pageSavePending]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.isComposing) return;
      const target = e.target;
      const closesOpenSearch =
        e.key === "Escape"
        && mobileSearchOpen
        && target === searchInputRef.current;
      if (
        target instanceof Element
        && target.closest(
          'input, textarea, select, [contenteditable]:not([contenteditable="false"])',
        )
        && !closesOpenSearch
      ) {
        return;
      }
      if (e.key === "/" && !e.metaKey && !e.ctrlKey) {
        if (pageSavePending) return;
        e.preventDefault();
        setMobileSearchOpen(true);
        searchInputRef.current?.focus();
      }
      if (e.key === "Escape") {
        if (e.defaultPrevented) return;
        if (responsiveSidebar.presentation === "overlay" && responsiveSidebar.open) return;
        if (query) {
          setQuery("");
        } else if (mobileSearchOpen) {
          setMobileSearchOpen(false);
        } else if (view.kind === "page-draft") {
          // PageDraftEditor owns Escape so it can await the same flush gate as Back.
          return;
        } else if (view.kind === "entity" || view.kind === "memory" || view.kind === "settings" || view.kind === "import" || view.kind === "graph" || view.kind === "page" || view.kind === "space" || view.kind === "distill-review") {
          navigateBack();
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [mobileSearchOpen, pageEditDirty, pageSavePending, query, responsiveSidebar.open, responsiveSidebar.presentation, view]);

  // Close dropdowns on outside click
  const handleEntityClick = (entityId: string) => {
    if (entityId === "__create_profile__") {
      navigateTo({ kind: "settings", section: "general" });
    } else if (entityId.startsWith("memory:")) {
      navigateTo({ kind: "memory", sourceId: entityId.replace("memory:", "") });
    } else if (entityId.startsWith("page:")) {
      navigateTo({ kind: "page", pageId: entityId.replace("page:", "") });
    } else {
      navigateTo({ kind: "entity", entityId });
    }
  };

  const openSearchResult = async (result: SearchResult) => {
    setQuery("");
    setMobileSearchOpen(false);
    const target = searchResultTarget(result);
    if (target.kind === "page") {
      navigateTo({ kind: "page", pageId: target.pageId });
    } else if (target.kind === "file") {
      try {
        await openSearchResultTarget(target.url);
      } catch (err) {
        toast.error(String(err));
      }
    } else {
      navigateTo({ kind: "memory", sourceId: result.source_id });
    }
  };

  return (
    <WorkspaceNavigationProvider>
    <div
      className="memory-shell flex h-screen w-full flex-col"
      style={{ backgroundColor: "var(--mem-bg)", color: "var(--mem-text)" }}
    >
      {/* Full-width header */}
      <header
        className="memory-workspace-header relative flex items-center gap-3 shrink-0"
        style={{
          height: MAIN_HEADER_HEIGHT,
          paddingLeft: topBarLeftInset(),
          paddingRight: "var(--workspace-header-right-padding, 20px)",
          background: "var(--mem-bg)",
        }}
        data-tauri-drag-region
      >
        <SidebarToggleButton collapsed={responsiveSidebar.collapsed} onToggle={responsiveSidebar.toggle} ref={sidebarToggleRef} />
        <div className="workspace-history-navigation" role="group" aria-label={t("main.historyNavigation")}>
          <button type="button" aria-label={t("main.back")} title={t("main.back")}
            className="workspace-history-button" disabled={!canNavigateBack}
            onClick={navigateBack}><ArrowLeft size={18} aria-hidden="true" /></button>
          <button type="button" aria-label={t("main.forward")} title={t("main.forward")}
            className="workspace-history-button" disabled={!viewForward.length}
            onClick={navigateForward}><ArrowRight size={18} aria-hidden="true" /></button>
        </div>
        {(!standardSidebarMounted || !responsiveSidebar.open) && <ReviewEnvironmentBadge compact />}
        <div className="flex-1" data-tauri-drag-region />

          {/* Right actions */}
          <div className="flex items-center gap-1.5 shrink-0">
            <button
              aria-expanded={mobileSearchOpen}
              aria-label={t("main.searchButton")}
              className="lg:hidden rounded-md p-1.5 transition-colors duration-150 hover:bg-[var(--mem-hover-strong)]"
              disabled={pageSavePending}
              onClick={() => {
                if (!pageSavePending) setMobileSearchOpen((open) => !open);
              }}
              style={{ color: "var(--mem-text-secondary)" }}
              type="button"
            >
              <svg aria-hidden="true" fill="none" height="16" stroke="currentColor" viewBox="0 0 24 24" width="16">
                <path d="m21 21-4.35-4.35m2.35-5.65a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" />
              </svg>
            </button>
            <ActivityStatus
              current={view.kind === "activity"}
              expanded={activityOpen}
              onToggle={toggleActivity}
              onOpenActivity={() => {
                navigateTo({ kind: "activity" });
              }}
              onOpenIntelligence={() => navigateTo({ kind: "settings", section: "intelligence" })}
            />
            {/* Quick Capture */}
            <button
              onClick={() => void invoke("open_quick_capture", { placement: "centered-over-main" })}
              className="p-1.5 rounded-md transition-colors duration-150 hover:bg-[var(--mem-hover-strong)]"
              style={{ color: "var(--mem-text-secondary)" }}
              title={t("main.quickCaptureTitle")}
            >
              <svg className="w-[16px] h-[16px]" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7" />
                <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z" />
              </svg>
            </button>
          </div>

          {/* Search — absolutely centered */}
          <div
            className={`${mobileSearchOpen ? "flex" : "hidden"} absolute left-4 right-4 top-[56px] z-50 items-center lg:flex lg:left-1/2 lg:right-auto lg:top-auto lg:-translate-x-1/2`}
          >
            <div
              className="flex w-full items-center gap-2 rounded-md px-3 py-[6px] shadow-lg focus-within:outline focus-within:outline-2 focus-within:outline-offset-2 focus-within:outline-[var(--mem-accent-page)] lg:w-[clamp(220px,40vw,480px)] lg:shadow-none"
              style={{
                backgroundColor: "var(--mem-sidebar)",
                border: "1px solid var(--mem-border)",
              }}
            >
            <svg className="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" style={{ color: "var(--mem-text-tertiary)" }}>
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
            <input
              ref={searchInputRef}
              data-wenlan-search-input
              disabled={pageSavePending}
              value={displayedSearchQuery}
              onChange={(e) => handleSearchQueryChange(e.target.value)}
              placeholder={t("main.searchPlaceholder")}
              className="flex-1 bg-transparent outline-none"
              style={{
                fontFamily: "var(--mem-font-body)",
                fontSize: "13px",
                color: "var(--mem-text)",
              }}
              spellCheck={false}
              autoComplete="off"
            />
            {displayedSearchQuery && (
              <button
                onClick={() => {
                  pendingDraftSearchCancelRef.current?.();
                  pendingDraftSearchCancelRef.current = null;
                  setPendingDraftSearchQuery(null);
                  setQuery("");
                }}
                className="shrink-0"
                style={{ color: "var(--mem-text-tertiary)" }}
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 6L6 18M6 6l12 12" />
                </svg>
              </button>
            )}
            </div>
          </div>
      </header>

      {/* Sidebar + Content row */}
      <div className="flex flex-1 overflow-hidden">
        {view.kind === "settings" ? (
          <SettingsSidebar
            collapsed={sidebarCollapsed}
            active={view.section ?? "general"}
            onSelect={(section) => navigateTo({ kind: "settings", section })}
          />
        ) : view.kind === "connect-agent" ? null : (
          <Sidebar
            activeNavigation={activeNavigation}
            collapsed={responsiveSidebar.collapsed}
            inventoryScope={view.kind === "pages" || view.kind === "page" || view.kind === "page-draft" ? view.inventoryScope : undefined}
            browsingPages={view.kind === "pages" || view.kind === "home"}
            onBrowsePages={(inventoryScope) => navigateTo({ kind: "pages", inventoryScope })}
            currentPageId={view.kind === "page" ? view.pageId : view.kind === "page-draft" ? view.draftId : null}
            currentMemoryId={view.kind === "memory" ? view.sourceId : null}
            onSelectMemory={(sourceId) => navigateTo({ kind: "memory", sourceId })}
            currentSpaceId={view.kind === "space" ? view.spaceId : null}
            onEntityClick={handleEntityClick}
            onNavigateLog={() => navigateTo({ kind: "stream" })}
            onNavigatePages={navigateNotes}
            onCreatePage={(folderPath) => navigateTo({ kind: "page-draft", space: null, folderPath, inventoryScope: view.kind === "pages" ? view.inventoryScope : undefined })}
            onNavigateEntities={() => navigateTo({ kind: "entities" })}
            onNavigateHome={navigateHome}
            onNavigateGraph={() => navigateTo({ kind: "graph" })}
            onNavigateSources={() => navigateTo({ kind: "sources" })}
            onNavigateSpaces={navigateSpaces}
            onNavigateSettings={() => navigateTo({ kind: "settings", section: "general" })}
            onOpenAbout={() => setAboutOpen(true)}
            onRequestClose={responsiveSidebar.close}
            onSelectDraft={(draftId, space) => navigateTo({ kind: "page-draft", draftId, space, inventoryScope: view.kind === "pages" || view.kind === "page" || view.kind === "page-draft" ? view.inventoryScope : undefined })}
            onSelectPage={(page) => navigateTo({ kind: "page", pageId: page.id, inventoryScope: view.kind === "pages" || view.kind === "page" || view.kind === "page-draft" ? view.inventoryScope : undefined })}
            onSelectSpace={(space) => navigateTo({ kind: "space", spaceId: space.id, spaceName: space.name })}
            open={responsiveSidebar.open}
            presentation={responsiveSidebar.presentation}
            recentPagesRevision={recentPagesRevision}
            recentSpacesRevision={recentSpacesRevision}
          />
        )}

        {/* Main content */}
        <main ref={mainContentRef} className={`flex-1 ${view.kind === "graph" || view.kind === "sources" ? "min-w-0 overflow-hidden p-0" : `memory-main-content overflow-y-auto${(view.kind === "page" || view.kind === "memory") && !query ? " memory-main-content--page" : ""}`}`}>
          {/* Search results overlay */}
          {query ? (
            (memoryResults.length > 0 || sourceResults.length > 0 || entityResults.length > 0 || conceptResults.length > 0) ? (
              <div className="flex flex-col gap-2">
                {conceptResults.length > 0 && (
                  <>
                    <p
                      style={{
                        fontFamily: "var(--mem-font-mono)",
                        fontSize: "11px",
                        color: "var(--mem-text-tertiary)",
                      }}
                    >
                      {t("main.search.pages")}
                    </p>
                    {conceptResults.map((c) => (
                      <div
                        key={c.id}
                        className="rounded-lg px-4 py-3 cursor-pointer transition-colors duration-150 hover:bg-[var(--mem-hover)]"
                        style={{ backgroundColor: "var(--mem-surface)", border: "1px solid var(--mem-border)" }}
                        onClick={() => { setQuery(""); navigateTo({ kind: "page", pageId: c.id }); }}
                      >
                        <div className="flex items-center gap-2.5">
                          <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: "var(--mem-accent-page)" }} />
                          <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", fontWeight: 500, color: "var(--mem-text)" }}>
                            {c.title}
                          </span>
                          {c.domain && (
                            <span style={{ fontFamily: "var(--mem-font-mono)", fontSize: "10px", color: "var(--mem-text-tertiary)" }}>
                              {c.domain}
                            </span>
                          )}
                        </div>
                        {c.summary && (
                          <p
                            style={{
                              fontFamily: "var(--mem-font-body)",
                              fontSize: "12px",
                              color: "var(--mem-text-secondary)",
                              marginTop: 4,
                              marginLeft: 16,
                              lineHeight: 1.5,
                            }}
                          >
                            {c.summary}
                          </p>
                        )}
                      </div>
                    ))}
                  </>
                )}
                {memoryResults.length > 0 && (
                  <>
                    <p
                      style={{
                        fontFamily: "var(--mem-font-mono)",
                        fontSize: "11px",
                        color: "var(--mem-text-tertiary)",
                        marginTop: conceptResults.length > 0 ? 12 : 0,
                      }}
                    >
                      {t("main.search.memories", { count: memoryResults.length, query })}
                    </p>
                    {memoryResults.map((r) => (
                      <MemorySearchResult key={r.id} result={r} query={query} onClick={() => void openSearchResult(r)} />
                    ))}
                  </>
                )}
                {sourceResults.length > 0 && (
                  <>
                    <p
                      style={{
                        fontFamily: "var(--mem-font-mono)",
                        fontSize: "11px",
                        color: "var(--mem-text-tertiary)",
                        marginTop: (memoryResults.length > 0 || conceptResults.length > 0) ? 12 : 0,
                      }}
                    >
                      {t("main.search.sources")}
                    </p>
                    {sourceResults.map((r) => (
                      <MemorySearchResult key={r.id} result={r} query={query} onClick={() => void openSearchResult(r)} />
                    ))}
                  </>
                )}
                {entityResults.length > 0 && (
                  <>
                    <p
                      style={{
                        fontFamily: "var(--mem-font-mono)",
                        fontSize: "11px",
                        color: "var(--mem-text-tertiary)",
                        marginTop: (memoryResults.length > 0 || sourceResults.length > 0 || conceptResults.length > 0) ? 12 : 0,
                      }}
                    >
                      {t("main.search.entities")}
                    </p>
                    {entityResults.map((r) => (
                      <div
                        key={r.entity.id}
                        className="rounded-lg px-4 py-3 cursor-pointer transition-colors duration-150 hover:bg-[var(--mem-hover)]"
                        style={{ backgroundColor: "var(--mem-surface)", border: "1px solid var(--mem-border)" }}
                        onClick={() => { setQuery(""); handleEntityClick(r.entity.id); }}
                      >
                        <div className="flex items-center gap-2.5">
                          <span
                            className="text-[10px] font-medium px-1.5 py-0.5 rounded-full"
                            style={{ backgroundColor: "color-mix(in srgb, var(--mem-accent-sage) 15%, transparent)", color: "var(--mem-accent-sage)" }}
                          >
                            {r.entity.entity_type}
                          </span>
                          <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "13px", fontWeight: 500, color: "var(--mem-text)" }}>
                            {r.entity.name}
                          </span>
                          {r.entity.domain && (
                            <span style={{ fontFamily: "var(--mem-font-mono)", fontSize: "10px", color: "var(--mem-text-tertiary)" }}>
                              {r.entity.domain}
                            </span>
                          )}
                        </div>
                      </div>
                    ))}
                  </>
                )}
              </div>
            ) : (
              <p
                style={{
                  fontFamily: "var(--mem-font-mono)",
                  fontSize: "11px",
                  color: "var(--mem-text-tertiary)",
                }}
              >
                {t("main.search.noResults", { query })}
              </p>
            )
          ) : view.kind === "import" ? (
            <ImportView
              onBack={navigateBack}
              completeLabel={view.fromFirstUse ? t("firstUse.guide.seeKnowledge") : undefined}
              onComplete={(_source, result) => {
                if (view.fromFirstUse) {
                  showView({
                    kind: "first-use",
                    showKnowledge: true,
                    batchId: result.imported > 0 ? result.batch_id : undefined,
                  });
                  updateHistory([{ kind: "pages" }], []);
                } else {
                  replaceView({ kind: "stream" });
                }
              }}
            />
          ) : view.kind === "settings" ? (
            <SettingsPage
              currentSpace={contextSpace?.spaceName}
              section={view.section ?? "general"}
              onBack={navigateBack}
              onSetupAgent={() => navigateTo({ kind: "connect-agent" })}
              onImport={() => navigateTo({ kind: "import" })}
            />
          ) : (view.kind === "pages" || view.kind === "home") ? (
            <PagesOverview
              inventoryScope={view.kind === "pages" ? view.inventoryScope : undefined}
              onBrowseAll={() => navigateTo({ kind: "pages" })}
              onBrowseFolder={(inventoryScope) => navigateTo({ kind: "pages", inventoryScope })}
              onOpenReview={() => navigateTo({ kind: "distill-review" })}
              onCreatePage={(space, folderPath) => navigateTo({ kind: "page-draft", space, folderPath, inventoryScope: view.kind === "pages" ? view.inventoryScope : undefined })}
              onSelectDraft={(draftId, space) => navigateTo({
                kind: "page-draft",
                draftId,
                space,
                inventoryScope: view.kind === "pages" ? view.inventoryScope : undefined,
              })}
              onSelectPage={(id) => navigateTo({ kind: "page", pageId: id, inventoryScope: view.kind === "pages" ? view.inventoryScope : undefined })}
              onSelectSpace={(spaceName) => navigateTo({ kind: "space", spaceId: null, spaceName })}
            />
          ) : view.kind === "entities" ? (
            <EntitiesView
              onEntityClick={handleEntityClick}
            />
          ) : view.kind === "spaces" ? (
            <SpacesOverview
              createIntent={view.create}
              labels={spacesOverviewLabels}
              onCreateIntentHandled={() => replaceView({ kind: "spaces" })}
              onSelectSpace={(spaceName) => navigateTo({ kind: "space", spaceId: null, spaceName })}
              onSpaceDeleted={handleSpaceDeleted}
              onSpaceRenamed={handleSpaceRenamed}
            />
          ) : view.kind === "space" ? (
            <SpaceDetail
              copy={spaceDetailCopy}
              spaceName={view.spaceName}
              onBack={() => replaceView({ kind: "spaces" })}
              onCreatePage={(space) => navigateTo({ kind: "page-draft", space })}
              onReviewAll={() => navigateTo({ kind: "distill-review" })}
              onSelectPage={(id) => navigateTo({ kind: "page", pageId: id })}
              onSpaceDeleted={handleSpaceDeleted}
              onSpaceLoaded={handleSpaceLoaded}
              onSpaceRenamed={handleSpaceRenamed}
            />
          ) : view.kind === "memory" ? (
            <MemoryDetail
              key={view.sourceId}
              onRegisterNavigationGuard={registerMemoryNavigationGuard}
              sourceId={view.sourceId}
              onBack={navigateBack}
              onNavigateEntity={handleEntityClick}
              onNavigateMemory={(sid) => navigateTo({ kind: "memory", sourceId: sid })}
              onNavigatePage={(pageId) => navigateTo({ kind: "page", pageId })}
            />
          ) : view.kind === "connect-agent" ? (
            <SetupWizard
              initialStep="connect"
              onComplete={navigateBack}
            />
          ) : view.kind === "entity" ? (
            <EntityDetail
              key={view.entityId}
              entityId={view.entityId}
              onBack={navigateBack}
              onEntityClick={handleEntityClick}
              onMemoryClick={(sid) => navigateTo({ kind: "memory", sourceId: sid })}
              onPageClick={(pageId) => navigateTo({ kind: "page", pageId })}
            />
          ) : view.kind === "page-draft" ? (
            <PageDraftEditor
              key={view.sessionKey ?? view.draftId ?? "new"}
              draftId={view.draftId}
              onBack={() => {
                if (responsiveSidebar.presentation === "overlay" && responsiveSidebar.open) {
                  responsiveSidebar.close();
                  return;
                }
                navigateBack();
              }}
              onEscapeBeforeLeave={() => {
                if (
                  responsiveSidebar.presentation === "overlay"
                  && responsiveSidebar.open
                ) {
                  responsiveSidebar.close();
                  return true;
                }
                return false;
              }}
              onOpenExisting={(pageId) => {
                afterPageDraftFlush(() => replaceView({ kind: "page", pageId }));
              }}
              onPublished={(pageId, projectionIssue) => replaceView({ kind: "page", pageId, inventoryScope: view.inventoryScope, projectionIssue })}
              ref={pageDraftEditorRef}
              folderPath={view.folderPath ?? (view.inventoryScope ? inventoryFolderPath(view.inventoryScope) ?? undefined : undefined)}
              space={view.space}
            />
          ) : view.kind === "page" ? (
            <PageDetail
              pageId={view.pageId}
              projectionIssue={view.projectionIssue}
              onProjectionResolved={() => replaceView({ ...view, projectionIssue: undefined })}
              initialSelection={pageSelectionsRef.current.get(view.pageId)}
              onSelectionChange={(selection) => pageSelectionsRef.current.set(view.pageId, selection)}
              onEditorReady={() => setReadyEditorView(view)}
              initialMode={view.mode ?? "edit"}
              onBack={navigateBackFromPageDetail}
              onRegisterFlush={registerPageFlush}
              onEditDirtyChange={setPageEditDirty}
              onMemoryClick={(sid) => navigateTo({ kind: "memory", sourceId: sid })}
              onPageLoaded={handlePageLoaded}
              onPageClick={(id) => navigateTo({ kind: "page", pageId: id })}
              onEntityClick={handleEntityClick}
              onSavePendingChange={setPageSavePending}
            />
          ) : view.kind === "distill-review" ? (
            <DistillReviewPanel
              onBack={navigateBack}
              onPageClick={(id) => navigateTo({ kind: "page", pageId: id, mode: "read" })}
              onMemoryClick={(sid) => navigateTo({ kind: "memory", sourceId: sid })}
            />
          ) : view.kind === "first-use" ? (
            <FirstUseGuide
              onBack={navigateBack}
              initialView={view.showKnowledge ? "live" : "guide"}
              batchId={view.batchId}
              onImport={() => navigateTo({ kind: "import", fromFirstUse: true })}
              onSources={() => navigateTo({ kind: "settings", section: "sources" })}
              onConnect={(client) => navigateTo(client === "codex" || client === "claude"
                ? { kind: "connect-agent" }
                : { kind: "settings", section: "agents" })}
              onOpenIntelligence={() => navigateTo({ kind: "settings", section: "intelligence" })}
              onOpenPage={(id) => {
                updateHistory([...viewHistoryRef.current, {
                  kind: "first-use",
                  showKnowledge: true,
                  batchId: view.batchId,
                }], []);
                showView({ kind: "page", pageId: id });
              }}
            />
          ) : view.kind === "activity" ? (
            <ActivityFeed
              onNavigateMemory={(sid) => navigateTo({ kind: "memory", sourceId: sid })}
              onOpenIntelligence={() =>
                navigateTo({ kind: "settings", section: "intelligence" })
              }
            />
          ) : view.kind === "recaps" ? (
            <RecapsList
              onBack={navigateBack}
              onNavigateMemory={(sid) => navigateTo({ kind: "memory", sourceId: sid })}
            />
          ) : view.kind === "sources" ? (
            <SourcesView
              libraryState={sourceLibraryState}
              onLibraryStateChange={setSourceLibraryState}
              onManageSources={() => navigateTo({ kind: "settings", section: "sources" })}
            />
          ) : view.kind === "graph" ? (
            <div style={{ position: "relative", width: "100%", height: "100%" }}>
              <AtlasView
                onNodeClick={(target) =>
                  target.kind === "memory"
                    ? navigateTo({ kind: "memory", sourceId: target.id })
                    : target.kind === "page"
                      ? navigateTo({ kind: "page", pageId: target.id })
                      : handleEntityClick(target.id)
                }
                onBack={navigateBack}
              />
            </div>
          ) : (
            <>
              <WorkspaceBackButton onClick={() => navigateTo({ kind: "pages" })} className="p-1.5 -ml-1.5 rounded-md transition-colors duration-150 hover:bg-[var(--mem-hover)] mb-3" style={{ color: "var(--mem-text-tertiary)", background: "none", border: "none", cursor: "pointer", lineHeight: 0 }}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M19 12H5M12 19l-7-7 7-7" /></svg>
              </WorkspaceBackButton>
              <div className="mb-3 flex items-center justify-between gap-3">
                <h2 style={{ fontFamily: "var(--mem-font-heading)", fontSize: "24px", fontWeight: 500, color: "var(--mem-text)", margin: 0 }}>{t("main.memories")}</h2>
                <button
                  className="rounded-md px-2.5 py-1.5 transition-colors duration-150 hover:bg-[var(--mem-hover)]"
                  onClick={() => navigateTo({ kind: "recaps" })}
                  style={{
                    background: "transparent",
                    border: "1px solid var(--mem-border)",
                    color: "var(--mem-text-secondary)",
                    cursor: "pointer",
                    fontFamily: "var(--mem-font-body)",
                    fontSize: "12px",
                  }}
                  type="button"
                >
                  {t("main.recaps")}
                </button>
              </div>
              <MemoryStream
                memories={memories}
                selectedDomain={null}
                sortMode={sortMode}
                onSortChange={setSortMode}
                stabilityFilter={stabilityFilter}
                onStabilityFilterChange={setStabilityFilter}
                onSelectMemory={(sid) => navigateTo({ kind: "memory", sourceId: sid })}
                presentation="parent-list"
              />
            </>
          )}
        </main>
      </div>

      <AboutWenlanDialog open={aboutOpen} onClose={() => setAboutOpen(false)} />
      <QuickCaptureScrim />
    </div>
    </WorkspaceNavigationProvider>
  );
}
